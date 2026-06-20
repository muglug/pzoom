<?php

function foo(string $s): void {
    // Under ensureArrayIntOffsetsExist, destructuring a non-empty-list<string>
    // reports the offsets the list doesn't guarantee. explode(...) proves only
    // offset 0, so Psalm flags $b (offset 1) with PossiblyUndefinedIntArrayOffset
    // but leaves $a alone. The targets are still typed string.
    [$a, $b] = explode(":", $s);
    echo $a;
    echo $b;
}
