<?php
interface I {
    /**
     * @psalm-assert null|!ExpectedType $value
     */
    public static function foo($value);
}
