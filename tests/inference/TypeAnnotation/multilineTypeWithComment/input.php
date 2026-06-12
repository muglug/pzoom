<?php
/**
 * @psalm-type PhoneType = array{
 *    phone: string
 * }
 *
 * Bar
 */
class Foo {
    /** @var PhoneType */
    public static $phone;
}
$output = Foo::$phone;
