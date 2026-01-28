<?php
/**
 * @template T as array<bool>
 */
abstract class Foo {
    /**
     * @return value-of<T>
     */
    abstract public function getRandomValue(): string;
}
                
