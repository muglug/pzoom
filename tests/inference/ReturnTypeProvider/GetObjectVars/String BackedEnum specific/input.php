<?php
enum A: string { case One = "one"; case Two = "two"; }
function getBackedEnum(): A { return A::One; }
$b = get_object_vars(getBackedEnum());
