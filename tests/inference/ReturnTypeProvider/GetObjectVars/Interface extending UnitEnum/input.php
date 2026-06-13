<?php
interface A extends UnitEnum {}
enum B implements A { case One; }
function getA(): A { return B::One; }
$b = get_object_vars(getA());
