<?php
/**
 * @param mixed $v
 * @psalm-pure
 * @psalm-assert int $v
 */
function assertInt($v):void {
    if (!is_int($v)) {
        throw new \RuntimeException();
    }
}

/**
 * @psalm-pure
 * @param mixed $i
 */
function takesMixed($i) : int {
    assertInt($i);
    return $i;
}
