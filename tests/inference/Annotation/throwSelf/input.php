<?php
namespace Foo;

class MyException extends \Exception {
    /**
     * @throws self
     */
    public static function create(): void {
        throw new self();
    }
}
