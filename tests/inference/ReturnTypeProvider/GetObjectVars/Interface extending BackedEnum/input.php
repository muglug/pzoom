<?php
interface A extends BackedEnum {}
enum B: int implements A { case One = 1; }
function getA(): A { return B::One; }
$b = get_object_vars(getA());
