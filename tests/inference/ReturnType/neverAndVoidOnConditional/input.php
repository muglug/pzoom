<?php
/**
 * @template T as bool
 * @param T $end
 * @return (T is true ? never : void)
 */
function a($end): void{
    if($end){
        die();
    }
}
