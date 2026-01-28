<?php
class A { }
class_alias("A", "A_A");

echo A_A::class;
