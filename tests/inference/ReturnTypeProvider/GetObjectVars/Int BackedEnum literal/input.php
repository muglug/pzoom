<?php
enum A: int { case One = 1; case Two = 2; }
$b = get_object_vars(A::One);
