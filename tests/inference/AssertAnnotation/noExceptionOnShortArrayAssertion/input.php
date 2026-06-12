<?php
/**
 * @param mixed[] $a
 */
function one(array $a): void {
  isInts($a);
}

/**
 * @psalm-assert int[] $value
 * @param mixed $value
 */
function isInts($value): void {}
