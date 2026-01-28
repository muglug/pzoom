<?php
/**
 * @template T
 * @param callable(T) $x
 * @param array<T> $y
 */
function example($x, $y): void {}

example(
    /**
     * @param int|false $x
     */
    function($x): void {},
    [rand(0, 1) ? 5 : false]
);