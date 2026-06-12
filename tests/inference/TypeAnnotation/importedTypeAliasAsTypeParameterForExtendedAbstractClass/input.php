<?php
namespace Bar;

/** @template T */
abstract class A {}

/** @psalm-type Foo=string */
class B {}

/**
 * @psalm-import-type Foo from B
 * @extends A<Foo>
 */
class C extends A {}
