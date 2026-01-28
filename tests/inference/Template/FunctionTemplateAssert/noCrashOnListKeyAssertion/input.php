<?php
/**
 * @template T
 * @param T $t
 * @param mixed $other
 * @psalm-assert =T $other
 */
function assertSame($t, $other) : void {}

/** @param list<int> $list */
function takesList(array $list) : void {
    foreach ($list as $i => $l) {
        assertSame($i, $l);
    }
}