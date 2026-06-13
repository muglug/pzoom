<?php
/**
 * @param object{a:int, b:string, c:bool} $p
 * @return array{a:int, b:string, c:bool}
 */
function f(object $p): array {
    return get_object_vars($p);
}
