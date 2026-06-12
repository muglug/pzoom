<?php
namespace Bar;

/** @template T */
abstract class A {}

/** @psalm-type Foo=string */
class B {}

/**
 * @psalm-import-type Foo from B as NewName
 * @extends A<NewName>
 */
class C extends A {}
