<?php
enum A { case One; case Two; }
$b = get_object_vars(A::One);
