<?php
/**
 * @psalm-inheritors FooClass|BarClass
 */
interface InterfaceA {}
/**
 * @psalm-inheritors FooClass|BarClass
 */
interface InterfaceB {}
class FooClass implements InterfaceA, InterfaceB {}
