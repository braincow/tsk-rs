# Task management utility

Task managemed done "right".

## tsk

An utility to create and manage known tasks or their states. Also can be used to track time spent on any given task with `start` and `stop` subcommands.

### Task descriptor language

Simply defines properties and metadata of the task:

`This is a prj:Project task that has to be done. due:2022-08-01T16:00:00 prio:low meta:x-fuu=bar tag:some tag:tags tag:can tag:be tag:added`

And this descriptor can be fed to the `tsk new` command.

Not all or any descriptors need to be fleshed and task is created still with just description filled. Other values can be set with `tsk set` subcommand.

### Special tags

| Tag  	    | Description  	|
|---	    |---	        |
| hold  	| When present removes points from score to lower the urgency of the task. |
| next  	| When present increases points to score to make it the very first on your task list. |
| start  	| Only effective during `tsk new`. When present automatically starts timetracking. |

## tsknt

An utility to add Markdown formatted notebook to your task. When leaving a task to work on another just leave yourself a note on what you did so it becomes a tad easier to pickup from where you left off.

To assist in organizing your work it is possible to write todo lists into the notes by using Markdown Github style of extensions, like this:

- [ ] this action point has not been completed
- [x] this action however has been

These action points can then be queried with `tsknt action-points` subcommand.
