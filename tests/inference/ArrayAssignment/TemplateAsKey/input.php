<?php

class Foo {

    /**
     * @psalm-template T of array
     * @param T $offset
     * @param array<array, string> $weird_array
     */
    public function getThisName($offset, $weird_array): string {
        return $weird_array[$offset];
    }
}
