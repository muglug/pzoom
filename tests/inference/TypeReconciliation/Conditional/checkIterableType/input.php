<?php
/**
 * @param array<int> $x
 */
function takesArray (array $x): void {}

/** @var iterable<int> */
$x = null;
assert(is_array($x));
takesArray($x);

/**
 * @param Traversable<int> $x
 */
function takesTraversable (Traversable $x): void {}

/** @var iterable<int> */
$x = null;
assert($x instanceof Traversable);
takesTraversable($x);