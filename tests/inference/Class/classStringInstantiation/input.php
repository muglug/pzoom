<?php
class Foo {}
class Bar {}
$class = mt_rand(0, 1) === 1 ? Foo::class : Bar::class;
$object = new $class();
