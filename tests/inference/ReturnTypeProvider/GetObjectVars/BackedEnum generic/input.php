<?php
enum A: int { case One = 1; case Two = 2; }
function getBackedEnum(): BackedEnum { return A::One; }
$b = get_object_vars(getBackedEnum());
