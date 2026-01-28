<?php
class one { public function two(string $_p): void {} }
array_map(["two", "three"], ["one", "two"]);
