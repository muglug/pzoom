<?php
class A {}
class B extends A {}

$b = get_parent_class(new A());
if ($b === false) {}
$c = new $b();
