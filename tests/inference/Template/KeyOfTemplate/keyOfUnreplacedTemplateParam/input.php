<?php
/**
 * @template T as array<string, bool>
 */
abstract class Foo {
    /**
     * @return key-of<T>
     */
    abstract public function getRandomKey(): string;
}
                
