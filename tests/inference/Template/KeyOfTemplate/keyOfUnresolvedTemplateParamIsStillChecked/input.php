<?php
/**
 * @template T as array<int, bool>
 */
abstract class Foo {
    /**
     * @return key-of<T>
     */
    abstract public function getRandomKey(): string;
}
                
