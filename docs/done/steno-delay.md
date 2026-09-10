This task describes a new feature called steno delay. The changes themselves will be made in a
combination of the bbq-keyboard directory and the jolt-embassy-rp directory.

Right now, when the keyboard is operating in steno mode, strokes come in and are given to the
barbecue keyboard layer, which then routes them to the steno translation part, where they are fed
into the NFA that runs the dictionary lookup. Each stroke fed in results in a structure that comes
out and contains some number of keys to delete where a previous completion was incomplete or
incorrect, followed by some number of keys to type.

This happens, for example, with a word where the base word has to have some change made in order to
incorporate an ending, for example, removing or doubling letters when adding suffixes. Sometimes
they're much more complicated. These are always sent immediately as key presses, which can result in
the computer seeing words typed and then backspace to remove things with additional things typed.
Although this usually works, there are some environments that don't interpret backspace
consistently, such as browser completion in the browser address bar, and overall, it looks kind of
weird.

The purpose of this task is to insert a delay in this process, where the results that come out of
the system are delayed, in a sense, and not immediately typed, with a configurable delay. I'm going
to guess something like half a second would be a good starting point for this. Any that come before
a previous one has been typed can be effectively combined, and the backspace consumes things that
were typed by previous entries. With the caveat that sometimes it will have to actually type them if
the typing is done slow enough or the user's entry of strokes is done slow enough that the words
come in and get typed and they have to be backspaced for a correction. When the entry is fast
enough, things will be corrected before they are typed, and the user will just see the typing of the
end result.
