ljobs
=====

A tool to execute commands in parallel (something like GNU parallel).

Written to try out the [Little language](http://www.little-lang.org/).

Usage
-----

    ljobs [OPTIONS...] COMMAND [CMD-ARGS...] ::: TASKS...
    ljobs [OPTIONS...] COMMAND [CMD-ARGS...] < TASKS

ljobs will run the given command for each task.  Multiple commands can
run in parallel.

Each *task* is an arbitrary string, commonly a file name or other input.
If the `:::` form is used then tasks are given directly on the command
line.  Otherwise, tasks are read from standard input, one line per task.

These strings are replaced in command arguments:

    {}      replaced by the task argument
    {.}     replaced by task without extension
    {/}     replaced by basename of task
    {//}    replaced by dirname of task
    {/.}    replaced by basename of task without extension
    {#}     replaced by the task number, counting from 0

If none of the strings occur in a command argument then the task is
appended as the last argument of the command, i.e. `{}` is implied.

The command is executed *without* invoking a shell unless the `-c`
option is used.

Options
-------

These are the options to ljobs itself.

  * `-j NUM`, `--jobs NUM`

    Specify number of job slots. Defaults to number of processors
    detected.

  * `-k`, `--keep-going`

    Continue starting tasks even if a previous task failed.

  * `-c`

    Execute *command* with the shell interpreter given by the `SHELL`
    environment variable, or else `/bin/sh`.  The command arguments are
    passed as positional parameters $1, $2, etc.

  * `-v`, `--verbose`

    Enable verbose output.

  * `--dry-run`

    Print commands to be executed but do not run them.

  * `-h`, `--help`

    Show usage message.

  * `--`

    End option processing.

Output buffering
----------------

The standard output and standard error outputs of a running command are
buffered in temporary files, and only output once the command stops.
This prevents interleaving of outputs for different tasks.

Exit status
-----------

With `-k` or `--keep-going` the exit status is

    0       all tasks executed successfully
    1-253   number of failed tasks
    254     more than 253 failed tasks
    255     other error

Without `-k` or `--keep-going` the exit status is

    0       all tasks executed successfully
    1-255   exit status of a failed task

Examples
--------

    ljobs oggenc ::: *.flac

    ljobs lame {} {.}.mp3 ::: *.wav

Bugs
----

  * Does not catch signals. This seems to require a Tcl extension.

  * Command arguments cannot begin with "|". This appears to be a
    bug in the implementation of L's `spawn` function.

Author
------

Peter Wang
