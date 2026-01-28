<?php
class A {}
class AChild extends A {}

/**
 * @template T
 * @param callable(T):void $c1
 * @param callable(T):void $c2
 * @param T $a
 */
function foo(callable $c1, callable $c2, $a): void {
  $c1($a);
  $c2($a);
}

foo(
  function(AChild $_a) : void {},
  function(AChild $_a) : void {},
  new A()
);
