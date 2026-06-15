<?php
namespace Ns;

/**
 * @template T
 * @param class-string<T> $className
 * @param callable(T):void $outmaker
 * @return T
 */
function createProxy(
    string $className,
    callable $outmaker
) : object {
    $t = new $className();
    $outmaker($t);
    return $t;
}

class A {
    public function bar() : void {}
}

function foo(A $o):void {}

createProxy(A::class, 'Ns\foo')->bar();