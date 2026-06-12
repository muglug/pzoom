<?php
/**
 * @param  mixed $value
 * @psalm-assert !array $value
 */
function myAssertNotArray($value) : void {}

 /**
 * @param  mixed $value
 * @psalm-assert !iterable $value
 */
function myAssertNotIterable($value) : void {}

/**
 * @param  int|array $v
 */
function takesIntOrArray($v) : int {
    myAssertNotArray($v);
    return $v;
}

/**
 * @param  int|iterable $v
 */
function takesIntOrIterable($v) : int {
    myAssertNotIterable($v);
    return $v;
}
