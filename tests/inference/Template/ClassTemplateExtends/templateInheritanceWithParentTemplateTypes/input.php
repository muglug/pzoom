<?php
/**
 * @template T1
 */
class A {
    /**
     * @template T2
     * @param class-string<T2> $t
     * @return ?T2
     */
    public function get($t) {
        return new $t;
    }
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class AChild extends A {
    /**
     * @template T3
     * @param class-string<T3> $t
     * @return ?T3
     */
    public function get($t) {
        return new $t;
    }
}
