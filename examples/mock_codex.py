#!/usr/bin/env python3
"""Synthetic app-server protocol fixture. No auth files, model calls or network."""
import fcntl,json,os,sys,time,uuid
state_file='concurrency.json'
thread_id='';turn_id=''
def send(value):
 print(json.dumps(value),flush=True)
def change(delta):
 with open(state_file,'a+') as file:
  fcntl.flock(file,fcntl.LOCK_EX);file.seek(0)
  state=json.loads(file.read() or '{"active":0,"maximum":0,"calls":0}')
  state['active']+=delta;state['maximum']=max(state['maximum'],state['active'])
  if delta>0:state['calls']+=1
  file.seek(0);file.truncate();json.dump(state,file);file.flush()
  return state
for line in sys.stdin:
 message=json.loads(line);method=message.get('method');params=message.get('params',{})
 if 'id' not in message:continue
 value={}
 if method=='config/read':value={'config':{'mcp_servers':{'untrusted_fixture':{}}}}
 elif method=='thread/start':
  assert params['ephemeral'] and params['sandbox']=='read-only'
  assert params['config']['mcp_servers']['untrusted_fixture']['enabled']==False
  assert params['config']['features']['shell_tool']==False
  thread_id=str(uuid.uuid4());value={'thread':{'id':thread_id}}
 elif method=='mcpServerStatus/list':value={'data':[],'nextCursor':None}
 elif method=='turn/start':
  assert params['threadId']==thread_id
  turn_id=str(uuid.uuid4());value={'turn':{'id':turn_id}}
 send({'id':message['id'],'result':value})
 if method=='turn/start':
  change(1);deadline=time.monotonic()+6
  while time.monotonic()<deadline:
   state=change(0)
   if state['maximum']==3:break
   time.sleep(.025)
  time.sleep(.06)
  text=params['input'][0]['text']
  for kind in ['contextCompaction','subAgentActivity']:
   send({'method':'item/started','params':{'threadId':thread_id,'turnId':turn_id,'item':{'id':str(uuid.uuid4()),'type':kind,'kind':'completed','agentPath':'/fixture','agentThreadId':'fixture-child'}}})
  if text=='NATIVE_TOOL_ATTEMPT':
   send({'method':'item/started','params':{'threadId':thread_id,'turnId':turn_id,'item':{'id':str(uuid.uuid4()),'type':'commandExecution'}}})
  send({'method':'item/completed','params':{'threadId':thread_id,'turnId':turn_id,'item':{'type':'agentMessage','phase':'final_answer','text':text}}})
  send({'method':'turn/completed','params':{'threadId':thread_id,'turn':{'id':turn_id,'status':'completed'}}})
  change(-1)
