/*
** ljobs - A tool to execute commands in parallel.
*/

extern crate getopts;
extern crate num_cpus;

use getopts::Options as Getopt;
use std::cmp::min;
use std::env;
use std::fmt;
use std::io::{self, Read, Write, Result};
use std::os::unix::process::ExitStatusExt;
use std::process::{exit, Command, Stdio, Child, ExitStatus};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/*---------------------------------------------------------------------------*/

const PROG: &'static str = "ljobs";

struct Options {
    maxjobs:    usize,
    keepgoing:  bool,
    shell:      Option<String>,
    verbose:    bool,
    dryrun:     bool
}

struct Job {
    tasknum:    usize,
    quotedcmd:  String,
    child:      Child,
    waitresult: Result<ExitStatus>
}

/*---------------------------------------------------------------------------*/

fn warn(args: fmt::Arguments) {
    io::stderr().write_fmt(args).expect("Could not write to stderr");
}

fn die(args: fmt::Arguments) -> ! {
    warn(args);
    exit(255);
}

macro_rules! warn {
    ( $( $x:expr ),+ ) => { warn(format_args!( $( $x ),+ )) }
}

macro_rules! die {
    ( $( $x:expr ),+ ) => { die(format_args!( $( $x ),+ )) }
}

/*---------------------------------------------------------------------------*/

fn process_options(argv: &Vec<String>) -> (Options, Vec<String>) {

    let mut getopt = Getopt::new();
    getopt.optflagmulti("h", "help", "print this help menu");
    getopt.optopt("j", "jobs", "number of job slots", "NUM");
    getopt.optflagmulti("k", "keep-going", "keep going even if a task failed");
    getopt.optflag("c", "", "run shell command");
    getopt.optflagmulti("v", "verbose", "verbose output");
    getopt.optflagmulti("n", "dry-run", "print commands but do not run them");
    getopt.parsing_style(getopts::ParsingStyle::StopAtFirstFree);

    let matches = match getopt.parse(&argv[1..]) {
        Ok(m) => m,
        Err(err) => die!("{}\n", err)
    };

    if matches.opt_present("h") {
        usage(getopt);
        exit(255);
    }

    let mut opts = Options {
        maxjobs:    0,
        keepgoing:  false,
        shell:      None,
        verbose:    false,
        dryrun:     false
    };

    if let Some(s) = matches.opt_str("j") {
        opts.maxjobs = s.parse().unwrap_or(0);
        if opts.maxjobs < 1 {
            die!("invalid argument for --jobs\n");
        }
    }
    if opts.maxjobs < 1 {
        opts.maxjobs = num_cpus::get();
    }

    opts.keepgoing = matches.opt_present("k");

    if matches.opt_present("c") {
        match env::var("SHELL") {
            Ok(val) =>
                opts.shell = Some(val),
            Err(env::VarError::NotPresent) =>
                opts.shell = Some(String::from("/bin/sh")),
            Err(env::VarError::NotUnicode(_)) =>
                die!("SHELL value not Unicode\n")
        }
    }

    opts.verbose = matches.opt_present("v");

    opts.dryrun = matches.opt_present("n");

    return (opts, matches.free);
}

fn usage(getopt: Getopt) {
    let head = vec![
        "Usage:\n",
        "    ljobs [OPTIONS...] COMMAND [CMD-ARGS...] ::: TASKS...\n",
        "    ljobs [OPTIONS...] COMMAND [CMD-ARGS...] < TASKS"
    ];
    let tail = vec![
        "String substitutions in command arguments:\n",
        "    {}                  task\n",
        "    {.}                 task without extension\n",
        "    {/}                 basename of task\n",
        "    {//}                dirname of task\n",
        "    {/.}                basename of task without extension\n",
        "    {#}                 task number\n",
        "\n"
    ];

    for x in head {
        print!("{}", x);
    }
    print!("{}\n", getopt.usage(""));
    for x in tail {
        print!("{}", x);
    }
}

/*---------------------------------------------------------------------------*/

fn main() {
    // Possibly we should work with OsStrings but getopts does not support
    // OsStrings for now so we would need to switch to another option parser.
    let argv = std::env::args().collect();
    let (opts, freeargs) = process_options(&argv);

    if freeargs.len() == 0 || freeargs[0] == ":::" {
        die!("no command\n");
    }
    let cmd = &freeargs[0];

    let (cmdargs, taskargs, taskstdin);
    match freeargs.iter().position(|x| x == ":::") {
        Some(i) => {
            cmdargs = &freeargs[1..i];
            taskargs = &freeargs[i+1..];
            taskstdin = false;
        },
        None => {
            cmdargs = &freeargs[1..];
            taskargs = &[];
            taskstdin = true;
        }
    };

    let (errs, failedexit) = master(&opts, cmd, cmdargs, taskstdin, taskargs);

    exit(
        if opts.keepgoing {
            min(254, errs as i32)
        } else if errs > 0 {
            failedexit as i32
        } else {
            0
        }
    );
}

fn master(opts: &Options,
          cmd: &String,
          cmdargs: &[String],
          taskstdin: bool,
          taskargs: &[String]) -> (u32, i32) {

    let mut numjobs = 0;
    let mut tasknum = 0;
    let mut errs = 0;
    let mut failedexit = 255;

    // The Rust standard library does not provide a way to wait on multiple
    // child processes at once. Therefore we spawn a thread to wait on each
    // individual child process then communicate the result back to the parent
    // through a channel.
    let (tx, mut rx) = mpsc::channel();

    'main: loop {
        let taskarg: String;
        if taskstdin {
            let mut line = String::new();
            match io::stdin().read_line(&mut line) {
                Ok(0) => // eof
                    break 'main,
                Ok(_) => {
                    chomp(&mut line);
                    taskarg = line;
                },
                Err(err) => {
                    die!("error reading standard input: {}\n", err);
                }
            }
        } else {
            if tasknum >= taskargs.len() {
                break 'main;
            }
            taskarg = taskargs[tasknum].clone();
        }

        let argv = build_argv(&opts, cmd, cmdargs, tasknum, &taskarg);
        let quotedcmd = quote_cmd(&argv);

        if opts.dryrun {
            dryrun(tasknum, &quotedcmd);
        } else {
            let mut command = Command::new(&argv[0]);
            command.args(&argv[1..]);
            command.stdin(Stdio::null());
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            if opts.verbose {
                warn!("{}[{}]: start\t{}\n", PROG, tasknum, quotedcmd);
            }

            match command.spawn() {
                Ok(mut child) => {
                    numjobs += 1;
                    let thread_tx = tx.clone();
                    thread::spawn(move || {
                        let res = child.wait();
                        let job = Job {
                            tasknum: tasknum,
                            quotedcmd: quotedcmd,
                            child: child,
                            waitresult: res
                        };
                        match thread_tx.send(job) {
                            Ok(_) => (),
                            Err(err) => die!("send error: {}\n", err)
                        }
                    });
                },
                Err(err) => {
                    warn!("{}[{}]: error\t{}: {}\n",
                          PROG, tasknum, quotedcmd, err);
                    errs += 1;
                }
            }
        }

        if numjobs >= opts.maxjobs {
            wait_jobs(&opts, &mut numjobs, &mut rx, false,
                      &mut errs, &mut failedexit);
        }

        if errs > 0 && !opts.keepgoing {
            break;
        }

        tasknum += 1;
    }

    wait_jobs(&opts, &mut numjobs, &mut rx, true, &mut errs, &mut failedexit);
    return (errs, failedexit);
}

/*---------------------------------------------------------------------------*/

fn build_argv(opts: &Options,
              cmd: &String,
              cmdargs: &[String],
              tasknum: usize,
              task: &String) -> Vec<String> {

    let mut argv: Vec<String> = Vec::new();
    let mut havetask = false;

    match opts.shell {
        Some(ref shell) => {
            argv.push(shell.clone());
            argv.push(String::from("-c"));
            argv.push(cmd.clone());
            argv.push(String::from("-"));
        },
        None => {
            argv.push(cmd.clone());
        }
    };

    for arg in cmdargs {
        match subst(arg, tasknum, task) {
            Some(substarg) => {
                argv.push(substarg);
                havetask = true;
            },
            None => {
                argv.push(arg.clone());
            }
        }
    }

    if !havetask {
        argv.push(task.clone());
    }

    return argv;
}

fn subst(s: &str, tasknum: usize, task: &str) -> Option<String> {
    let mut acc = String::new();
    let mut ss = s;
    let mut found = false;

    while ss.len() > 0 {
        if let Some(open) = ss.find('{') {
            if let Some(close0) = ss[open..].find('}') {
                acc.push_str(&ss[..open]);
                let close = open + close0;
                let mid = &ss[open+1..close];
                let next;
                match mid {
                    "" => {
                        acc.push_str(task);
                        next = close+1;
                        found = true;
                    },
                    "." => {
                        acc.push_str(remove_extension(task));
                        next = close+1;
                        found = true;
                    },
                    "/" => {
                        acc.push_str(basename(task));
                        next = close+1;
                        found = true;
                    },
                    "//" => {
                        acc.push_str(dirname(task));
                        next = close+1;
                        found = true;
                    },
                    "/." => {
                        acc.push_str(remove_extension(basename(task)));
                        next = close+1;
                        found = true;
                    },
                    "#" => {
                        acc.push_str(&tasknum.to_string());
                        next = close+1;
                        found = true;
                    },
                    _ => {
                        acc.push_str("{");
                        next = open+1;
                    }
                }
                ss = &ss[next..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    acc.push_str(ss);

    if found {
        Some(acc)
    } else {
        None
    }
}

/*
fn subst(s: &str, tasknum: usize, task: &str) -> String {

    // Using regex as Rust standard library does not provide a simple way to
    // perform multiple string replacement. It could be written more directly.
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"\{\}|\{\.\}|\{/\}|\{//\}|\{/\.\}|\{#\}").unwrap();
    }
    let subster = Subster {
        tasknum: tasknum,
        task: task
    };
    return RE.replace_all(s, subster);
}

struct Subster<'a> {
    tasknum: usize,
    task: &'a str
}

impl<'a> regex::Replacer for Subster<'a> {

    fn reg_replace(&mut self, caps: &regex::Captures) -> Cow<str> {
        match caps.at(0) {
            Some("{}") => Borrowed(&self.task),
            Some("{.}") => Borrowed(remove_extension(&self.task)),
            Some("{/}") => Borrowed(basename(&self.task)),
            Some("{//}") => Borrowed(dirname(&self.task)),
            Some("{/.}") => Borrowed(remove_extension(basename(&self.task))),
            Some("{#}") => Owned(self.tasknum.to_string()),
            Some(s) => panic!("unexpected capture {}", s),
            None => panic!("unexpected capture None")
        }
    }

    fn no_expand(&mut self) -> Option<Cow<str>> {
        None
    }
}
*/

// std::path is too subtle...

fn basename(s: &str) -> &str {
    match s.rfind('/') {
        None => s,
        Some(i) => &s[i+1..]
    }
}

fn extension(s: &str) -> Option<&str> {
    let base = basename(s);
    match base.rfind('.') {
        None => None,
        Some(i) => Some(&base[i..]) // including dot
    }
}

fn remove_extension(s: &str) -> &str {
    match extension(s) {
        None => s,
        Some(ext) => &s[..s.len()-ext.len()]
    }
}

fn dirname(s: &str) -> &str {
    let s = remove_redundant_trailing_slashes(s);
    match s.rfind('/') {
        None => ".",
        Some(0) => "/",
        Some(i) => remove_redundant_trailing_slashes(&s[..i])
    }
}

// Remove trailing slashes but not a leading slash.
fn remove_redundant_trailing_slashes(s: &str) -> &str {
    if s.len() > 1 && s.ends_with('/') {
        remove_redundant_trailing_slashes(&s[..s.len()-1])
    } else {
        s
    }
}

fn chomp(s: &mut String) {
    if s.ends_with('\n') {
        let n = s.len() - 1;
        s.truncate(n);
    }
}

/*---------------------------------------------------------------------------*/

fn quote_cmd(args: &Vec<String>) -> String {
    let v: Vec<String> = args.iter().map(quote_arg).collect();
    return v.join(" ");
}

fn quote_arg(s: &String) -> String {
    if s == "" {
        String::from("''")
    } else if shell_safe_chars(s) {
        s.clone()
    } else {
        String::from("'") + &s.replace("'", "'\"'\"'") + "'"
    }
}

fn shell_safe_chars(s: &str) -> bool {
    for c in s.chars() {
        match c {
            'A'...'Z'|'a'...'z'|'0'...'9' => (),
            '_'|'%'|'+'|','|'-'|'.'|'/'|':'|'='|'@' => (),
            _ => return false
        }
    }
    return true;
}

/*---------------------------------------------------------------------------*/

fn dryrun(tasknum: usize, quotedcmd: &str) {
    print!("[{}]\t{}\n", tasknum, quotedcmd);
}

/*---------------------------------------------------------------------------*/

fn wait_jobs(opts: &Options,
             numjobs: &mut usize,
             rx: &mut Receiver<Job>,
             waitall: bool,
             errs: &mut u32,
             failedexit: &mut i32) {

    while *numjobs > 0 {
        match rx.recv() {
            Ok(ref mut job) => {
                *numjobs -= 1;
                done_job(opts, job, errs, failedexit);
            },
            Err(err) => {
                die!("recv error: {}\n", err);
            }
        }

        if !waitall {
            break;
        }
    }
}

fn done_job(opts: &Options,
            job: &mut Job,
            errs: &mut u32,
            failedexit: &mut i32) {

    if let Some(ref mut f) = job.child.stderr {
        show_output(&mut io::stderr(), f, job.tasknum, &job.quotedcmd,
            opts.verbose);
    }
    if let Some(ref mut f) = job.child.stdout {
        show_output(&mut io::stdout(), f, job.tasknum, &job.quotedcmd,
            false);
    }

    match job.waitresult {
        Ok(ref exitstatus) => {
            match exitstatus.code() {
                Some(0) => {
                    if opts.verbose {
                        warn!("{}[{}]: done\t{}\n",
                              PROG, job.tasknum, job.quotedcmd);
                    }
                },
                Some(exit) => {
                    if opts.verbose {
                        warn!("{}[{}]: exit {}\t{}\n",
                            PROG, job.tasknum, exit, job.quotedcmd);
                    }
                    *errs += 1;
                    *failedexit = exit;
                },
                None => {
                    match exitstatus.signal() {
                        Some(signal) => {
                            if opts.verbose {
                                warn!("{}[{}]: signal {}\t{}\n",
                                    PROG, job.tasknum, signal, job.quotedcmd);
                            }
                            *errs += 1;
                            *failedexit = 128 + signal;
                        },
                        None => {
                            // Should not happen.
                            panic!("child terminated for unknown reason");
                        }
                    }
                }
            }
        },
        Err(ref err) => {
            warn!("wait error pid {}: {}\n", job.child.id(), err);
            *errs += 1;
            *failedexit = 255;
        }
    }
}

fn show_output(out: &mut Write,
               inp: &mut Read,
               tasknum: usize,
               quotedcmd: &str,
               sep: bool) {

    let mut buf = Vec::new();
    match inp.read_to_end(&mut buf) {
        Ok(0) => (),
        Ok(_) => {
            if sep {
                checked_write_fmt(out,
                    format_args!("-------- {}[{}]: {} --------\n",
                        PROG, tasknum, quotedcmd));
            }
            checked_write_all(out, &buf);
            if sep {
                checked_write_fmt(out, format_args!("--------\n"));
            }
        },
        Err(err) => {
            die!("read error: {}\n", err)
        }
    }
}

fn checked_write_all(f: &mut Write, buf: &[u8]) {
    match f.write_all(buf) {
        Ok(_n) => (),
        Err(e) => die!("write error: {}\n", e)
    }
}

fn checked_write_fmt(f: &mut Write, args: fmt::Arguments) {
    match f.write_fmt(args) {
        Ok(_n) => (),
        Err(e) => die!("write error: {}\n", e)
    }
}

/*---------------------------------------------------------------------------*/
