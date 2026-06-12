<?php
/**
 * @param Closure(int):int $f
 * @param Closure(int):int $g
 *
 * @return Closure(int):int
 */
function foo(Closure $f, Closure $g) : Closure {
    return fn(int $x):int => $f($g($x));
}
