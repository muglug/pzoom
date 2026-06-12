<?php
function getObject() : iterable{
   return [];
}

$iterableObject = getObject();

if (is_countable($iterableObject)) {
   if (count($iterableObject) === 0) {}
}
