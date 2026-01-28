<?php
/**
 * @template T
 * @param class-string<T> $className
 * @param Closure(T):void $outmaker
 * @return T
 */
function createProxy(
    string $className,
    Closure $outmaker
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

createProxy(A::class, function(B $o):void {})->bar();
