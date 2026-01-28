<?php
/**
 * @template T of object
 */
class Foo {
    /**
     * @psalm-return callable(T): ?T
     */
    public function bar() {
        return
            /**
             * @param T $data
             * @return ?T
             */
            function($data) {
                $data = rand(0, 1) ? $data : null;
                return $data;
            };
    }
}