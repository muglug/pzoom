<?php
namespace Bar;

/** @template T of string */
interface A {}

/**
 * @psalm-type Foo = "foo"
 */
class B {}

/**
 * @psalm-import-type Foo from B
 * @implements A<Foo>
 */
class C implements A {}
