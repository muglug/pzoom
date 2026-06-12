<?php
/**
 * @psalm-inheritors FooClass
 */
interface InterfaceA {}
/**
 * @psalm-inheritors BarClass
 */
interface InterfaceB {}
class BazClass implements InterFaceA, InterFaceB {}
