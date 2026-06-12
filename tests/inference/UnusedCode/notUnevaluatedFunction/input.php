<?php
/** @return never */
function neverReturns(){
    die();
}
unrelated();
neverReturns();

function unrelated():void{
    echo "hello";
}
