<?php
namespace Barrr;

/**
 * @psalm-type _C=array{c:_CC}
 * @psalm-type _CC=float
 */
class A {
    /**
     * @param _C $arr
     */
    public function foo(array $arr) : void {}
}
