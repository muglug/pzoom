<?php

/**
 * `isset($list[n])` proves indices 0..n exist on a list (it stays a list and
 * the lower offsets become definite), so the fetches below are safe under
 * ensureArrayIntOffsetsExist — matching Psalm, which does not flag them.
 */
function fromParam(string $line): void {
    /** explode never returns an empty list, so this is a non-empty-list */
    $parts = explode(':', $line);
    if (isset($parts[1])) {
        echo $parts[0];
        echo $parts[1];
    }
}

/** @param list<string> $a */
function fromList(array $a): void {
    if (isset($a[2])) {
        echo $a[0];
        echo $a[1];
        echo $a[2];
    }
}
