<?php
/**
 * @psalm-inheritors BarClass&FooInterface
 */
class BaseClass {}
interface FooInterface {}
class BarClass extends BaseClass implements FooInterface {}
