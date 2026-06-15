<?php
namespace Foo;

class A {
    public function test1() : void {}
}

class Application {
    public function test2() : void {}
}

/**
 * @template T of object
 * @template TName as class-string<T>|null
 *
 * @psalm-param TName $className
 *
 * @psalm-return (TName is null ? Application : T)
 */
function app(?string $className = null) {
    if ($className === null) {
        return new Application();
    }

    /**
     */
    return new $className();
}

app(A::class)->test1();
app()->test2();