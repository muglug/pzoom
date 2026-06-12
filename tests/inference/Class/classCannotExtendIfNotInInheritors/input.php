<?php
/**
 * @psalm-inheritors FooClass|BarClass
 */
class BaseClass {}
class BazClass extends BaseClass {} // this is an error
