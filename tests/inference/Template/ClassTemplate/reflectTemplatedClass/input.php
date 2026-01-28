<?php
/** @template T1 of object */
class Foo {
    /**
     * @param class-string<T1> $a
     * @psalm-return ReflectionClass<T1>
     */
    public function reflection(string $a) {
        return new ReflectionClass($a);
    }
}