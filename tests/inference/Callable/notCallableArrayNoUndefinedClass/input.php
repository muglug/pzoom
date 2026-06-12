<?php
/**
 * @psalm-param array|callable $_fields
 */
function f($_fields): void {}

f(["instance_date" => "ASC", "start_time" => "ASC"]);
