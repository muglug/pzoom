<?php
/**
 * @psalm-inheritors FooClass|BarClass
 */
class BaseClass {}
class FooClass extends BaseClass {}
class BarClass extends FooClass {}
