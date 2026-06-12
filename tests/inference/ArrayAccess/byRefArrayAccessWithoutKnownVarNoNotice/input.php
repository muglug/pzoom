<?php
$a = new stdClass();
/** @psalm-suppress MixedPropertyFetch */
print_r([&$a->foo->bar]);
