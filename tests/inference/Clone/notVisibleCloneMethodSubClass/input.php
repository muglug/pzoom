<?php
class a {
    private function __clone() {}
}
class b extends a {}

clone new b;
