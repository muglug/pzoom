<?php
interface I {}
$anon_instance = new class implements I {};

class Props {
    /** @var class-string<I>[] */
    public $arr = [];
}

(new Props)->arr[] = get_class($anon_instance);
