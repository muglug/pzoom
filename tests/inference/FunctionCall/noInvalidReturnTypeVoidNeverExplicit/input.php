<?php
/**
 * @return void|never
 */
function foo() {
    if ( rand( 0, 10 ) > 5 ) {
        exit;
    }
}
