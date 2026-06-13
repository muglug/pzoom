<?php
class A extends Missing {}
function make(): A { return new A; }

make()->prop;
