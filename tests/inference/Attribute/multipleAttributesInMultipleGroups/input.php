<?php
#[Attribute]
class A {}
#[Attribute]
class B {}
#[Attribute]
class C {}
#[Attribute]
class D {}

#[A, B]
#[C, D]
class Foo {}
