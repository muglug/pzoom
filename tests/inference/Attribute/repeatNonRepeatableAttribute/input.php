<?php
#[Attribute]
class Foo {}

#[Foo, Foo]
class Baz {}
