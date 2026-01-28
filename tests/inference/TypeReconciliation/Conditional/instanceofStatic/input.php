<?php
abstract class Foo {
    /**
     * @return static[]
     */
    abstract public static function getArr() : array;

    /**
     * @return static|null
     */
    public static function getOne() {
        $one = current(static::getArr());
        return $one instanceof static ? $one : null;
    }
}