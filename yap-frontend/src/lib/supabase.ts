import { createClient } from '@supabase/supabase-js'

const supabaseUrl = 'https://eearwzqotpfoderpfrqx.supabase.co'
const supabaseKey = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVlYXJ3enFvdHBmb2RlcnBmcnF4Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3NDgyMTUwOTIsImV4cCI6MjA2Mzc5MTA5Mn0.BmnDrHtD-THaSLHO9VE2X-PO6B-z9OkbxzjeIinN6b8"

export const supabase = createClient(supabaseUrl, supabaseKey)
