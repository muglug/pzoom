<?php
final class C {
    /** @var string */
    public $prop = "val";
}
$ret = get_object_vars(new C);
