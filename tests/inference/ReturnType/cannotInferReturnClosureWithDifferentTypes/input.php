<?php
class A {}
class B {}
/**
 * @return callable(A): void
 */
function map(): callable {
    return function(B $v): void {};
}
