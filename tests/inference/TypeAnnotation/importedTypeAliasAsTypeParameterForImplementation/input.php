<?php
namespace Bar;

/** @template T */
interface A {}

/** @psalm-type Foo=string */
class B {}

/**
 * @psalm-import-type Foo from B
 * @implements A<Foo>
 */
class C implements A {}
