<?php
/** @psalm-suppress InvalidScalarArgument */
#[Attribute("foobar")]
class Foo {}

#[Foo]
class Bar {}
