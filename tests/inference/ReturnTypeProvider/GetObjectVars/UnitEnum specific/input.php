<?php
enum A { case One; case Two; }
function getUnitEnum(): A { return A::One; }
$b = get_object_vars(getUnitEnum());
