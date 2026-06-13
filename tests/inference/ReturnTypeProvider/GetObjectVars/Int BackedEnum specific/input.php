<?php
enum A: int { case One = 1; case Two = 2; }
function getBackedEnum(): A { return A::One; }
$b = get_object_vars(getBackedEnum());
