# task

The `task` module runs functions **at the same time**: while one task waits (for the network, a timer, another task), the others go on. It is written in Koji (`std/task.kj`), on the tasks of the runtime (`std/koji/task.kj`).

```koji
import http
import task

immut urls = ['https://example.com', 'https://example.org']

// The two requests are sent at once, and the program waits for both.
immut pages = task.all([() => http.get(urls[0]), () => http.get(urls[1])])
```

## How Tasks Run

Tasks are light: each has a stack of its own (the system only gives it the memory it uses), so a program can have thousands of them. They take turns on the one thread of the program: when a task waits, the next task that is ready runs.

-   A task waits in `time.sleep`, in the requests and the server of `http`, in `Task.wait`, in the channels and shared values below, and in the calls that wait for the system: reading and writing files, listing directories, `input()`, `system.run` and `system.capture`, and finding the address of a host. While other tasks are alive, a thread that helps makes those calls, so they do not hold the others up.
-   A task that computes for long without waiting lets the others run every 10 milliseconds or so: a loop or a recursion never holds them up for longer. To use several processors, split the work into several tasks.
-   Only one task runs at a time, and a task changes only its own values: the values that tasks share are in channels and shared values, whose operations no other task comes between, so there are no data races.

The code of a task is ordinary code: there is no `async` or `await`, and a function does not need to know whether it runs in a task.

## Groups

A task always belongs to a **group**, and a group waits for its tasks before it ends, so no task outlives the code that started it.

-   `group(body: (Group) -> R) -> R`  
    Calls `body` with a new group, then waits for every task started in the group (by `body` or by those tasks): what `body` returned.

-   `Group.spawn(work: () -> T) -> Task[T]`  
    Starts a task that runs `work`. It runs when the current task waits (at the end of the group at the latest). Once the group has ended, it panics.

-   `Task.wait() -> T`  
    Waits for the task to end: what its function returned.

-   `Task.done() -> boolean`  
    Whether the task has ended.

-   `all(works: (() -> T)[]) -> T[]`  
    Runs each function in a task of its own, in one group, and returns their results in order.

```koji
import task
import time

immut results = task.group((tasks) => {
    immut slow = tasks.spawn(() => {
        time.sleep(0.2)
        return 'slow'
    })
    immut fast = tasks.spawn(() => {
        time.sleep(0.1)
        return 'fast'
    })

    return [slow.wait(), fast.wait()]
})

print(results) // ['slow', 'fast'], after 0.2 seconds
```

The parameter of the function given to `group` needs no type: it is a `task.Group`, from the type `group` expects (and so for every anonymous function given where its type is known).

A panic in a task stops the whole program, as it does outside tasks.

## Sharing Data

A task gets a copy of the values its function uses, like any function: changing them changes only its copy. Tasks share data in two ways.

### Channels

A channel is a queue of values from tasks that send them to tasks that receive them.

-   `channel(capacity: int) -> Channel[T]`  
    A new channel that holds up to `capacity` values sent and not received yet. With `0`, each `send` waits until its value is received.

-   `Channel.send(value: T) -> void`  
    Sends a value, after waiting for room. A closed channel takes no value: it panics.

-   `Channel.receive() -> Option[T]`  
    The next value, after waiting for one, or `None` once the channel is closed and empty.

-   `Channel.close() -> void`  
    Closes the channel: the values already in it can still be received. Closing it again panics.

```koji
import task

immut jobs: task.Channel[int] = task.channel(10)

immut total = task.group((tasks) => {
    tasks.spawn(() => {
        for i in range(100) {
            jobs.send(i)
        }

        jobs.close()
    })

    immut sum = tasks.spawn(() => {
        mut sum = 0

        while (true) {
            match (jobs.receive()) {
                Some(n) => sum += n
                None => break
            }
        }

        return sum
    })

    return sum.wait()
})

print(total) // 4950
```

### Shared Values

A `Shared[T]` is a value that all its copies see: the one way tasks share a variable.

-   `shared(value: T) -> Shared[T]`  
    A new shared value.

-   `Shared.get() -> T`  
    A copy of the value.

-   `Shared.set(value: T) -> void`  
    Gives it a new value.

-   `Shared.update(change: (T) -> T) -> T`  
    Gives it the value `change` returns for the current one, and returns it. No other task changes the value until `change` returns, even if `change` waits; calling `set` or `update` of the same value inside `change` panics.

```koji
import task

immut visits = task.shared(0)

task.group((tasks) => {
    for i in range(100) {
        tasks.spawn(() => visits.update((n) => n + 1))
    }
})

print(visits.get()) // 100
```

`update` copies a large value (a list, a map) that it changes: to collect many values, have the tasks return them, or send them through a channel.

## Deadlocks

When every task waits for another one (a result, a channel, a shared value) and none waits for a timer or the network, none can go on: the program stops with a panic that says so.
