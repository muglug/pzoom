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
    /** @psalm-suppress MixedMethodCall */
    $t = new $className();
    $outmaker($t);
    return $t;
}

class A {
    public function bar() : void {}
}

class B {}

function foo(B $o):void {}

createProxy(A::class, 'Ns\foo')->bar();
