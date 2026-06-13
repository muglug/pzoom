<?php
/** @template T as object */
class Generic {}

/**
 * @template T
 * @param T $p
 * @return Generic<T>
 * @psalm-suppress InvalidReturnType
 */
function violate($p) {}
