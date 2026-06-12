<?php
enum A { case One; case Two; }
function getUnitEnum(): UnitEnum { return A::One; }
$b = get_object_vars(getUnitEnum());
