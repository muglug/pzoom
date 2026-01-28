<?php
/**
 * @param array<int> $a
 */
function process(array $a): void {
    assert(!empty($a));
    /** @psalm-suppress RedundantConditionGivenDocblockType */
    assert(is_int($a[0]));
}