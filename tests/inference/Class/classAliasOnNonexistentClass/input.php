<?php
if (!class_exists(\PHPUnit\Framework\TestCase::class)) {
    /** @psalm-suppress UndefinedClass */
    class_alias(\PHPUnit_Framework_TestCase::class, \PHPUnit\Framework\TestCase::class);
}

class T extends \PHPUnit\Framework\TestCase {

}
