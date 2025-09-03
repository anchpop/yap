-- Fix search_path for handle_new_user function
ALTER FUNCTION public.handle_new_user() SET search_path = public, pg_catalog;

-- Fix search_path for update_updated_at_column function
ALTER FUNCTION public.update_updated_at_column() SET search_path = public, pg_catalog;