<?php
$a = FFI::cdef(
   "typedef enum _zend_ffi_symbol_kind {
        ZEND_FFI_SYM_TYPE,
        ZEND_FFI_SYM_CONST = 2,
        ZEND_FFI_SYM_VAR,
        ZEND_FFI_SYM_FUNC
    } zend_ffi_symbol_kind;"
);
echo $a->ZEND_FFI_SYM_TYPE;
echo $a->ZEND_FFI_SYM_CONST;
echo $a->ZEND_FFI_SYM_VAR;
                
