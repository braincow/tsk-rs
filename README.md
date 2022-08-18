# Task management utility

Task managemed done "right".

## tsk

An utility to create and manage known tasks or their states.

### Task descriptor language

Simply defines properties and metadata of the task:

`This is a prj:Project task that has to be done. due:2022-08-01T16:00:00 priority:low meta:fuu=bar`

And this descriptor can be fed to the `tsk new` command.

Not all or any descriptors need to be fleshed and task is created still with just description filled. Other values can be set with `tsk set` subcommand.

## tsknt

An utility to add Markdown formatted notebook to your task. When leaving a task to work on another just leave yourself a note on what you did so it becomes a tad easier to pickup from where you left off.
