<?php

/**
 * `count($list) >= n` proves offsets 0..n-1 exist (Psalm's
 * reconcileNonEmptyCountable list branch), so the fetches below are safe
 * under ensureArrayIntOffsetsExist — no PossiblyUndefinedIntArrayOffset.
 *
 * @param list<string> $a
 * @param non-empty-list<string> $b
 */
function narrows(array $a, array $b): void {
    if (count($a) > 1) {
        echo $a[0];
        echo $a[1];
    }

    // The `||` short-circuit narrows past the `count(...) < 2` guard.
    if (count($a) < 2) {
        return;
    }
    echo $a[0];
    echo $a[1];

    // Already non-empty: offset 0 is known, and `> 1` lifts offset 1 too.
    if (count($b) > 2) {
        echo $b[2];
    }
}
