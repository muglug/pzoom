<?php

function foo(string $s): void {
    // explode(...) with the default limit returns non-empty-list<string>, so
    // each destructured target is typed string. The list guarantees only its
    // first element, so $b (offset 1) is "possibly undefined" — but with the
    // default config (ensureArrayIntOffsetsExist off) that goes unreported,
    // matching Psalm.
    [$a, $b] = explode(":", $s);
    /** @psalm-check-type-exact $a = string */;
    /** @psalm-check-type-exact $b = string */;
}
