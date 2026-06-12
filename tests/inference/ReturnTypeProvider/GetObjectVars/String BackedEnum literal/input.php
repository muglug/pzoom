<?php
enum A: string { case One = "one"; case Two = "two"; }
$b = get_object_vars(A::One);
