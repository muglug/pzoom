<?php
/**
 * @template T
 *
 * @param Closure(T):void $t1
 * @param T $t2
 */
function apply(Closure $t1, $t2) : void {}

apply(function(int $_i) : void {}, 5);
apply(function(string $_i) : void {}, "hello");
apply(function(stdClass $_i) : void {}, new stdClass);

class A {}
class AChild extends A {}

apply(function(A $_i) : void {}, new AChild());