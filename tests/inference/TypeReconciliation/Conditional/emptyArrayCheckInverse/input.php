<?php
/**
 * @param non-empty-array $x
 */
function example(array $x): void {}

/** @var array */
$x = [];
if ($x === []) {
} else {
    example($x);
}